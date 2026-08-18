<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# Runtime Observability v1 — the `TOS.RUN.*` contract

Status: **Accepted Tier 2 interface contract.**

Its authority is assigned by `docs/38_NORMATIVE_DOCUMENT_HIERARCHY.md`, and it
conforms to the Tier 0 invariants and to every accepted Tier 1 ADR, which take
precedence over anything written here. Accepted by ADR-0042 (Project
Architect-approved, 2026-08-12), which also makes `TOS.RUN.*` the delegated
namespace Boot ABI v1 section 7 admits between its own success identifiers.

Producer: `source/crates/tos-pipeline` (`render::events`) and the component
driving it.
Consumers: the serial boot log, host test harnesses,
`source/host-tools/qemu-test/stage2-runtime.sh`.

## 1. Role

Boot ABI v1 (`interfaces/boot/BOOT_ABI_V1.md`) is the loader-to-nucleus
handoff: what the machine handed over and whether the capsule is what it
claims. It says nothing about *running* anything, because at the time it was
fixed nothing ran.

This contract covers the other question: what the TOS Core reference runtime
did with the canonical source it was given. It is a separate interface with a
separate vocabulary, because the two answer to different components and change
for different reasons. Boot ABI v1 is unchanged by this document.

## 2. What an event is

One line, terminated by CRLF on a serial transport, beginning with a stable
identifier matching `^TOS\.RUN\.[A-Z0-9_]+`, followed by space-separated
`key=value` fields. A value never contains whitespace: the producer substitutes
`_` for any whitespace or control character in a value that can carry free text
(a detail string, a rendered value), so a field can neither invent a field nor
split an event. Codes, digests, identifiers and numbers are never altered.

Identifiers and the fields listed as required below are stable. An
implementation MAY append further `key=value` fields after the required ones;
it MUST NOT remove one, reorder the required ones, or change what one means.

## 3. Progress

| Identifier | Required fields | Meaning |
|---|---|---|
| `TOS.RUN.BEGIN` | `path=` `bytes=` `entry=` `grant_base=0x<hex>` `grant_length=` `grant_version=`, and `modules=` when the run is over a source set | A run is starting over the named source with the named memory grant. |
| `TOS.RUN.STAGE` | `name=<read\|parse\|check\|resolve\|lower\|verify\|execute>` | The named stage is being **entered**. |

`modules` is an appended field under section 2's extension rule: `path` names
the entry module, and a run that resolved a set executed more than that. A log
showing only the entry would understate what ran.

`TOS.RUN.STAGE` is emitted before the stage runs, not after. A stage that never
returns is then named by the last event in the log, which is the only way a
hang or a trap inside a stage identifies itself from outside.

The stages appear in exactly this order, and the order is the reference path:
transport validity, grammar, checking, module-name-to-path agreement, lowering
to `tos-ir/v1`, independent verification, execution. A log in which they appear
in any other order, or in which one is missing before a later one, is not a
conforming run.

## 4. Outcome

Exactly one outcome event ends a run, and it is always the last of the run's
events.

| Identifier | Required fields | Meaning |
|---|---|---|
| `TOS.RUN.COMPLETED` | `value=` | The entry function returned. `value` is the rendered result. |
| `TOS.RUN.TRAP` | `code=` `at=` `detail=` | A defined dynamic failure ended the run. `at` is `path:line:column-line:column`, or `<unmapped>` when the IR carried no source map entry. |
| `TOS.RUN.REFUSED` | `stage=` and stage-specific fields | A stage refused to hand its output on. |

`TOS.RUN.REFUSED` carries the stage that refused and its reason:

| `stage=` | Further required fields |
|---|---|
| `read` | `code=` `byte=`, and `path=` when the run was over a source set |
| `parse`, `check`, `resolve` | `count=`, preceded by `count` × `TOS.RUN.DIAGNOSTIC` |
| `lower` | `construct=` `bytes=<start>..<end>` |
| `verify` | `code=` `at=` `detail=` |
| `execute` | `reason=` |

`TOS.RUN.DIAGNOSTIC` carries one frontend diagnostic in the docs/41 section 7
model: `<CODE> severity= stage= bytes=<start>..<end> at=<line>:<column>`, then
the module identity when the diagnostic carries one, then the diagnostic's
structured fields in their own order. The byte span is the normative locator;
line and column are derived from it.

`path` on a `read` refusal is an appended field under section 2's extension
rule, and it exists because a set has more than one unit: a byte offset with no
unit names nothing. A transport refusal never reaches a diagnostic, so nothing
else in the event can say which unit was refused.

A refusal by `parse`, `check` or `resolve` is a statement about the *program*.
A refusal by `lower` is a statement about the *lowerer* — the source is valid
and checked and this implementation cannot yet represent one of its constructs.
A refusal by `verify` is a statement about the *IR the frontend emitted*, which
is why it is never merged with the frontend's own diagnostics.

## 5. Verification and accounting

Emitted before the outcome on a completed run.

| Identifier | Required fields | Meaning |
|---|---|---|
| `TOS.RUN.VERIFIED` | `module=` `digest=sha256:<64 hex>` `verifier=` | The independent verifier issued a receipt for this exact module. |
| `TOS.RUN.ACCOUNTING` | `fuel=` `depth=` `tasks=` `allocation=` `cleanup=` `workers=`, each `used/limit` | What the run consumed against what the module declared. `shared=` and `sync=` are appended in the same form. |

`digest` is the complete `tos-ir/v1` module digest, and the engine ran the
module that digest names — a receipt for another module is refused before any
instruction executes (docs/43 section 5). A consumer that wants to know whether
verification really happened checks this event, not the absence of a refusal.

Every `used` in `TOS.RUN.ACCOUNTING` is bounded by its `limit`; a run that
exceeded one would have been stopped before the effect, so a log showing
otherwise is a defect and not a permitted state.

## 6. Implementation resources

Emitted after the outcome, by the component that supplied the memory.

| Identifier | Required fields | Meaning |
|---|---|---|
| `TOS.RUN.MEMORY` | `granted=` `peak=` `committed=` `blocks=` `free=` | The arena the run needed, against the region it was granted. |
| `TOS.RUN.STACK` | `used=` `capacity=` | Stack the run actually used, measured, against the region it ran on. |
| `TOS.RUN.TICKS` | `begin=` `end=` `waits=` | The monotonic tick the runtime read before and after the run. It counts timer interrupts (ADR-0049) and is not a duration: this contract carries no wall-clock time and no trusted time source. `waits` is how many reads the runtime made before the tick changed, and a run where it changed at all is a run the system interrupted and resumed. Absent when the system offers no tick. |
| `TOS.RUN.UNSTARTABLE` | `reason=` | The runtime could not be started at all. No stage ran. |

These are **implementation** figures and are never a statement about the module.
A module's `resource [allocation: ...]` is its own declared budget, enforced by
the engine and reported in `TOS.RUN.ACCOUNTING`; `TOS.RUN.MEMORY` is what the
implementation running it required. Exhausting one is not the other (ADR-0041).

`peak` is the highest address the arena was ever carried to, so it includes
block metadata, grain rounding, remainders too small to split off, and holes
below the frontier. It never falls when memory is freed: a bound must err
upward. `committed` is the live figure in whole blocks.

`TOS.RUN.STACK used` is measured, not estimated: the unused stack is painted
with an address-derived pattern before the run and read back after it.

## 7. Processes

A second producer emits under this namespace: the nucleus's process substrate.
`PROCESS_IDENTITY_V1` §6 delegates it here rather than to a namespace of its
own, because two vocabularies describing one system eventually disagree.

| Identifier | Required fields | Meaning |
|---|---|---|
| `TOS.RUN.PROCESS_BEGIN` | `module=` `runtime_engine=sha256:<64 hex>` `system_commit=` `asserted_by=launcher` | A process is being launched over the named module by the named runtime image. |
| `TOS.RUN.PROCESS_EXIT` | `asserted_by=nucleus` `self_reported_status=` | The process ended by saying so (`process_exit`, ADR-0054). |
| `TOS.RUN.PROCESS_FAULT` | `vector=` `error=0x<hex>` `rip=0x<hex>` `cr2=` `cpl=` | The process took a fault and ended. The system did not. |
| `TOS.RUN.PROCESS_RECLAIMED` | `frames=` `available=` | What the pool took back when the process ended, and what it holds now. |

Two fields carry their asserter in their name, and that is not decoration.
`asserted_by=nucleus` on an exit says the *fact* of the exit is the nucleus's;
`self_reported_status` says the number beside it is the process's claim about
its own work. A reader must never have to guess which kind of claim it is
holding (`PROCESS_IDENTITY_V1` §2), and merging the two would make the guess
necessary.

`system_commit=absent` is the true value for a capsule-launched Stage 3 process:
Stage 3 reads no repository, and writing a commit the system never read is the
failure Stage 1 was built to prevent.

The `TOS.RUN.*` events of §3–§6 are the *runtime's* — a process cannot reach a
serial port, so it writes them into the region its launch record names and the
nucleus relays them unchanged. Relaying is not authorship: the events say what
the runtime did, and the nucleus adds nothing to them.

## 8. Relationship to the boot log

When a runtime is driven from the nucleus, these events appear on the same
serial transport as the Boot ABI v1 events, between `TOS.IDENTITY` and
`TOS.HALT`. Boot ABI v1's own identifiers keep their relative order and their
meanings; nothing in this contract changes, removes or reinterprets any of them.

ADR-0042 settles the two questions this raised. Boot ABI v1's success
identifiers are a required **ordered subsequence** of the transport, and
identifiers of an accepted versioned interface contract — this one — MAY appear
between them. That is a delegation to an accepted contract, not an opening for
arbitrary unknown `TOS.*` namespaces: an identifier belonging to no accepted
contract is still unknown, and the Boot ABI rule for one is unchanged.

The Boot ABI terminal result remains authoritative for whether the boot
succeeded. Nothing in this contract reports a boot verdict; it reports what the
runtime did with the source it was given.

When the canonical boot module does not complete, the nucleus emits
`TOS.BOOTMODULE.FAIL` and halts with `RESULT_BOOT_MODULE_FAILED` (`0x25`),
whose exact condition Boot ABI v1 section 2 states. Which stage refused, and
why, is in the `TOS.RUN.*` events above — the result code says the boot module
failed, not how.
